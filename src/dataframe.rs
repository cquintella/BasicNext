// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Compatibility facade for the runtime-owned `DataFrame` primitives.

pub(crate) use bn_rt::{
    DataFrameJoin, DataFrameJoinConfig, DataFrameResource, add_dataframe_column, append_columns,
    append_rows, column_name, convert_dataframe_column, copy_dataframe_column,
    dataframe_reduce_column, duplicate_column_names, get_dataframe_cell, join_dataframes,
    select_dataframe, set_column_label, transpose_dataframe, zscore_column,
};
