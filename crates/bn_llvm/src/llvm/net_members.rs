// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

// HOST.Net members named by the text catalog (`net.txt`, `member.*`): the
// address predicates, the `CIDR` value, and TCP timeouts and half shutdown.
// A `CIDR` lowers like an `Address`, `{ i1, ptr, i64 }`: the network address
// text in field 1 and the prefix length in field 2.

use super::*;
use crate::ir::{CastOp, ICmpCond, InstSink, LlvmInst, LlvmOperand, LlvmType};
use crate::layout::handle_result_ty;
use runtime_abi::{
    NET_ADDRESS_IS_IPV4, NET_ADDRESS_IS_IPV6, NET_ADDRESS_IS_LINK_LOCAL, NET_ADDRESS_IS_LOOPBACK,
    NET_ADDRESS_IS_MULTICAST, NET_ADDRESS_IS_PRIVATE, NET_CIDR_CONTAINS, NET_CIDR_PARSE,
    NET_TCP_SET_TIMEOUTS, NET_TCP_SHUTDOWN_READ, NET_TCP_SHUTDOWN_WRITE,
};

/// Field of an `Address` or `CIDR` value that holds its address text.
const ADDRESS_TEXT_FIELD: usize = 1;
/// Field of a `CIDR` value that holds its prefix length.
const CIDR_PREFIX_FIELD: usize = 2;
/// Field of a stream value that holds its runtime handle.
const HANDLE_FIELD: usize = 2;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum NetMember {
    AddressIsIpv4,
    AddressIsIpv6,
    AddressIsLoopback,
    AddressIsPrivate,
    AddressIsLinkLocal,
    AddressIsMulticast,
    CidrParse,
    CidrContains,
    CidrNetwork,
    CidrPrefixLength,
    TcpSetTimeouts,
    TcpShutdownRead,
    TcpShutdownWrite,
}

/// Each member with the `net.txt` key of its contract name.
const MEMBERS: [(NetMember, &str); 13] = [
    (NetMember::AddressIsIpv4, "member.address.is_ipv4"),
    (NetMember::AddressIsIpv6, "member.address.is_ipv6"),
    (NetMember::AddressIsLoopback, "member.address.is_loopback"),
    (NetMember::AddressIsPrivate, "member.address.is_private"),
    (
        NetMember::AddressIsLinkLocal,
        "member.address.is_link_local",
    ),
    (NetMember::AddressIsMulticast, "member.address.is_multicast"),
    (NetMember::CidrParse, "member.cidr.parse"),
    (NetMember::CidrContains, "member.cidr.contains"),
    (NetMember::CidrNetwork, "member.cidr.network"),
    (NetMember::CidrPrefixLength, "member.cidr.prefix_length"),
    (NetMember::TcpSetTimeouts, "member.tcp.set_timeouts"),
    (NetMember::TcpShutdownRead, "member.tcp.shutdown_read"),
    (NetMember::TcpShutdownWrite, "member.tcp.shutdown_write"),
];

/// True for `HOST.Net.CIDR`, under its full or its imported name.
pub(crate) fn is_net_cidr_type(ty: &Type) -> bool {
    let full = bn_catalog::text("net", "type.cidr");
    let short = full.rsplit('.').next().unwrap_or(full);
    matches!(ty, Type::Named(name) if name == full || name == short)
        || matches!(ty, Type::ImportedNamed { name, .. } if name == short)
}

/// `ty` or `ty OR Error`: a narrowed value keeps the alternative's layout.
fn is_or_error(ty: &Type, member: fn(&Type) -> bool) -> bool {
    member(ty)
        || matches!(ty, Type::Alternative(alternatives)
            if alternatives.iter().any(member) && alternatives.iter().any(is_error_type))
}

impl NetMember {
    pub(crate) fn of(name: &str) -> Option<Self> {
        MEMBERS
            .iter()
            .find(|(_, key)| bn_catalog::text("net", key) == name)
            .map(|(member, _)| *member)
    }

    /// Whether the support check admits a call with these arguments.
    pub(crate) fn arguments_supported(
        self,
        arguments: &[ValueId],
        values: &HashMap<ValueId, Type>,
    ) -> bool {
        let argument = |index: usize| arguments.get(index).and_then(|value| values.get(value));
        let address =
            |index: usize| argument(index).is_some_and(|ty| is_or_error(ty, is_net_address_type));
        let cidr =
            |index: usize| argument(index).is_some_and(|ty| is_or_error(ty, is_net_cidr_type));
        let stream = |index: usize| {
            argument(index).and_then(llvm_type) == Some(crate::layout::handle_result_ty())
        };
        let integer = |index: usize| {
            argument(index)
                .and_then(llvm_type)
                .is_some_and(|ty| ty.is_integer())
        };
        match self {
            Self::AddressIsIpv4
            | Self::AddressIsIpv6
            | Self::AddressIsLoopback
            | Self::AddressIsPrivate
            | Self::AddressIsLinkLocal
            | Self::AddressIsMulticast => arguments.len() == 1 && address(0),
            Self::CidrParse => arguments.len() == 1 && argument(0) == Some(&Type::String),
            Self::CidrContains => arguments.len() == 2 && cidr(0) && address(1),
            Self::CidrNetwork | Self::CidrPrefixLength => arguments.len() == 1 && cidr(0),
            Self::TcpSetTimeouts => arguments.len() == 3 && stream(0) && integer(1) && integer(2),
            Self::TcpShutdownRead | Self::TcpShutdownWrite => arguments.len() == 1 && stream(0),
        }
    }

    pub(crate) fn lower(
        self,
        text: &mut crate::ir::LlvmModule,
        destination: ValueId,
        arguments: &[ValueId],
        analysis: &LoweringAnalysis<'_>,
    ) {
        let dest = destination.0;
        let reg = LlvmOperand::reg;
        let field = |text: &mut crate::ir::LlvmModule, label: &str, index: usize, at: usize| {
            let name = format!("{label}{dest}");
            text.assign(
                name.clone(),
                LlvmInst::extract(handle_result_ty(), value_reg(arguments[index]), at),
            );
            reg(name)
        };
        let boolean = |text: &mut crate::ir::LlvmModule, call: LlvmInst| {
            text.assign(format!("netholds{dest}"), call);
            text.assign(
                format!("v{dest}"),
                LlvmInst::icmp(
                    ICmpCond::Ne,
                    LlvmType::I32,
                    reg(format!("netholds{dest}")),
                    LlvmOperand::int(0),
                ),
            );
        };
        let i32_argument = |text: &mut crate::ir::LlvmModule, index: usize| {
            // The support check admitted only integer arguments here.
            let Some(ty) = analysis.values.get(&arguments[index]) else {
                unreachable!()
            };
            extend_to_i32(text, arguments[index], ty)
        };
        match self {
            Self::AddressIsIpv4
            | Self::AddressIsIpv6
            | Self::AddressIsLoopback
            | Self::AddressIsPrivate
            | Self::AddressIsLinkLocal
            | Self::AddressIsMulticast => {
                let address = field(text, "netaddr", 0, ADDRESS_TEXT_FIELD);
                let predicate = match self {
                    Self::AddressIsIpv4 => NET_ADDRESS_IS_IPV4,
                    Self::AddressIsIpv6 => NET_ADDRESS_IS_IPV6,
                    Self::AddressIsLoopback => NET_ADDRESS_IS_LOOPBACK,
                    Self::AddressIsPrivate => NET_ADDRESS_IS_PRIVATE,
                    Self::AddressIsLinkLocal => NET_ADDRESS_IS_LINK_LOCAL,
                    _ => NET_ADDRESS_IS_MULTICAST,
                };
                boolean(text, predicate.call([address]));
            }
            Self::CidrParse => {
                text.assign(format!("netout{dest}"), LlvmInst::alloca(LlvmType::Ptr));
                text.assign(format!("netprefix{dest}"), LlvmInst::alloca(LlvmType::I32));
                text.assign(
                    format!("netrc{dest}"),
                    NET_CIDR_PARSE.call([
                        value_reg(arguments[0]),
                        reg(format!("netout{dest}")),
                        reg(format!("netprefix{dest}")),
                    ]),
                );
                text.assign(
                    format!("netprefixv{dest}"),
                    LlvmInst::load(LlvmType::I32, reg(format!("netprefix{dest}"))),
                );
                text.assign(
                    format!("netprefixw{dest}"),
                    LlvmInst::cast(
                        CastOp::SExt,
                        LlvmType::I32,
                        reg(format!("netprefixv{dest}")),
                        LlvmType::I64,
                    ),
                );
                emit_net_result(
                    text,
                    destination,
                    &LlvmOperand::reg(format!("netrc{dest}")),
                    &LlvmOperand::reg(format!("netout{dest}")),
                    &LlvmOperand::reg(format!("netprefixw{dest}")),
                );
            }
            Self::CidrContains => {
                let network = field(text, "netcidr", 0, ADDRESS_TEXT_FIELD);
                let wide = field(text, "netcidrp", 0, CIDR_PREFIX_FIELD);
                let prefix = cidr_prefix(text, dest, wide);
                let address = field(text, "netaddr", 1, ADDRESS_TEXT_FIELD);
                boolean(text, NET_CIDR_CONTAINS.call([network, prefix, address]));
            }
            Self::CidrNetwork => {
                let network = field(text, "netcidr", 0, ADDRESS_TEXT_FIELD);
                text.assign(
                    format!("netaddr0{dest}"),
                    LlvmInst::insert(
                        handle_result_ty(),
                        LlvmOperand::undef(),
                        LlvmType::I1,
                        LlvmOperand::bool(false),
                        0,
                    ),
                );
                text.assign(
                    format!("netaddr1{dest}"),
                    LlvmInst::insert(
                        handle_result_ty(),
                        reg(format!("netaddr0{dest}")),
                        LlvmType::Ptr,
                        network,
                        1,
                    ),
                );
                text.assign(
                    format!("v{dest}"),
                    LlvmInst::insert(
                        handle_result_ty(),
                        reg(format!("netaddr1{dest}")),
                        LlvmType::I64,
                        LlvmOperand::int(0),
                        2,
                    ),
                );
            }
            Self::CidrPrefixLength => {
                let prefix = field(text, "netcidrp", 0, CIDR_PREFIX_FIELD);
                let wide =
                    analysis.values.get(&destination).and_then(llvm_type) == Some(LlvmType::I64);
                let inst = if wide {
                    LlvmInst::binary(BinaryOp::Add, LlvmType::I64, prefix, LlvmOperand::int(0))
                } else {
                    LlvmInst::cast(CastOp::Trunc, LlvmType::I64, prefix, LlvmType::I32)
                };
                text.assign(format!("v{dest}"), inst);
            }
            Self::TcpSetTimeouts => {
                let handle = field(text, "nethandle", 0, HANDLE_FIELD);
                let read = i32_argument(text, 1);
                let write = i32_argument(text, 2);
                emit_void_call_result(
                    text,
                    destination,
                    NET_TCP_SET_TIMEOUTS.call([handle, read, write]),
                );
            }
            Self::TcpShutdownRead | Self::TcpShutdownWrite => {
                let handle = field(text, "nethandle", 0, HANDLE_FIELD);
                let shutdown = if self == Self::TcpShutdownRead {
                    NET_TCP_SHUTDOWN_READ
                } else {
                    NET_TCP_SHUTDOWN_WRITE
                };
                emit_void_call_result(text, destination, shutdown.call([handle]));
            }
        }
    }
}

/// The prefix length of a `CIDR` (field 2, `i64`) as the `i32` the runtime takes.
fn cidr_prefix(text: &mut crate::ir::LlvmModule, dest: u32, prefix: LlvmOperand) -> LlvmOperand {
    let name = format!("netcidrp32{dest}");
    text.assign(
        name.clone(),
        LlvmInst::cast(CastOp::Trunc, LlvmType::I64, prefix, LlvmType::I32),
    );
    LlvmOperand::reg(name)
}
