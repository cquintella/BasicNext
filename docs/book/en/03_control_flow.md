# Control Flow

[← Previous: Common Programming Concepts](02_common_programming_concepts.md) · [Contents](toc.md)

A program whose statements run once, in written order, can only compute a fixed formula. Everything else — validating input, searching a table, retrying an operation, reporting a summary — requires the program to choose between alternatives and to repeat work until a condition holds. Those two capabilities are what this chapter covers.

Basic Next takes a specific position on how they are written. Every construct opens with a keyword and closes with an explicit `END`; every condition must be a `BOOLEAN`; every loop exit names the kind of loop it leaves. The cost is a few extra words per block. The return is that the shape of a program can be read from the source without reconstructing it from indentation, and that a set of defects common in other languages is rejected before the program runs. This chapter states each rule, shows the diagnostic the compiler emits when the rule is broken, and then shows how to structure decisions so that the rules rarely come into play.

## Conditions Are Boolean

A condition in Basic Next must have static type `BOOLEAN`. Integers, strings, pointers, and object references are never treated as true or false. The compiler rejects the program before execution:

```basic
LET pending AS INTEGER = 0

IF pending THEN
    PRINT "work remains"
END IF
```

```
error[TYPE_MISMATCH]: Type mismatch: Expected BOOLEAN, but found INTEGER(INT32) in condition.
```

The intent has to be written out, which also records which comparison was meant:

```basic
IF pending > 0 THEN
    PRINT "work remains"
END IF
```

Languages that accept a numeric condition must also decide what counts as false, and the decision differs between them: `0` is false in C, and so are the empty string and the empty list in Python, while `"0"` is true in one language and false in another. Programs that move between those conventions carry the question with them. Requiring `BOOLEAN` removes the question rather than answering it, and it makes a condition that tests a value against a threshold say which threshold.

When a condition expresses a rule of the problem domain rather than a mechanical test, give the rule a name by extracting a function that returns `BOOLEAN`:

```basic
FUNCTION IsOutOfRange(reading AS INTEGER, upperLimit AS INTEGER) AS BOOLEAN
    RETURN reading < 0 OR reading > upperLimit
END FUNCTION
```

The call site then reads as the rule it applies — `IF IsOutOfRange(reading, UPPER_LIMIT) THEN` — and the rule becomes testable on its own.

## The Block `IF`

The block form of `IF` evaluates a condition, runs one branch, and closes with `END IF`. Additional conditions are chained with `ELSE IF`, written as two words, and a final `ELSE` covers the remaining cases:

```basic
IF score >= 90 THEN
    PRINT "Grade: A"
ELSE IF score >= 80 THEN
    PRINT "Grade: B"
ELSE
    PRINT "Grade: C"
END IF
```

A chain is evaluated top to bottom and the first branch whose condition holds is the only one that runs. Order therefore carries meaning: conditions must be written from the most restrictive to the least, because a broader test placed first makes every narrower test below it unreachable. In the example above, moving `score >= 80` to the top would classify every grade of 90 or more as a B.

The `ELSE` branch is optional. Leaving it out states that no action is required when none of the conditions holds; supplying it and leaving it empty states the same thing less clearly, so prefer omission.

## The Single-Line `IF`

When a branch consists of one simple statement, the whole conditional may be written on one physical line, with no `END IF`:

```basic
IF retries > 3 THEN PRINT "giving up" ELSE PRINT "retrying"
```

The form is limited by design. Each branch admits one simple statement — a declaration, an assignment, a call, `PRINT`, `INPUT`, `RETURN`, `EXIT`, `CONTINUE`, or `STOP` — and the whole conditional must fit on one physical line. A compound statement such as a nested `IF` or a loop is rejected:

```
error[E0100]: Syntax error: single-line IF branches cannot contain compound statements
```

Adding `END IF` is likewise a syntax error, because the parser has already closed the statement at the end of the line:

```
error[E0100]: Syntax error: Expected expected END FUNCTION, found END IF in source parser.
```

Use this form for a guard that is fully expressed in one clause, such as `IF count > LIMIT THEN STOP 1`. Any branch that is likely to grow a second statement is better written as a block from the start, since converting it later touches three lines instead of one.

## Guard Clauses Instead of Nesting

A function that validates several preconditions before doing its work can be written by nesting the successful path inside each check:

```basic
FUNCTION Discount(total AS FLOAT, isMember AS BOOLEAN, coupon AS STRING) AS FLOAT
    IF total > 0.0 THEN
        IF isMember THEN
            IF coupon <> "" THEN
                RETURN total * 0.15
            ELSE
                RETURN total * 0.05
            END IF
        ELSE
            RETURN 0.0
        END IF
    ELSE
        RETURN 0.0
    END IF
END FUNCTION
```

The same function can be written by rejecting each invalid case as it is detected, leaving the successful path at the outermost level:

```basic
FUNCTION Discount(total AS FLOAT, isMember AS BOOLEAN, coupon AS STRING) AS FLOAT
    IF total <= 0.0 THEN
        RETURN 0.0
    END IF
    IF NOT isMember THEN
        RETURN 0.0
    END IF
    IF coupon = "" THEN
        RETURN total * 0.05
    END IF
    RETURN total * 0.15
END FUNCTION
```

The second version computes the same results, but each precondition and its consequence sit on adjacent lines, and the reader never has to hold an open branch in mind while reading the next one. This arrangement is usually called a guard clause. It applies whenever a function can answer early: validation, absent data, and error cases first, then the main work unindented at the end.

Basic Next gives this style a further incentive. Because each block closes with an explicit `END IF`, a deeply nested conditional spends several lines on closing markers alone, and the cost of nesting is visible in the source rather than hidden in indentation. When a chain of guards grows past four or five, the function is usually deciding two separate things; extracting one of them into a named function is the corrective step.

## Narrowing an Alternative Type

Values whose type is an alternative — `STRING OR EOF`, `File OR Error`, `Node OR NULL` — cannot be used as the primary type until the other case has been excluded. The `IS` operator performs that test, and when the branch that tests it ends in `RETURN` or `STOP`, the exclusion applies to the statements that follow the `IF`:

```basic
FUNCTION Start() AS VOID
    LET line AS STRING OR EOF = INPUT()
    IF line IS EOF THEN
        PRINT "no input"
        RETURN
    END IF
    PRINT "read", LEN(line), "characters"
END FUNCTION
```

After the `IF`, `line` has type `STRING`, so `LEN(line)` is accepted. Had the guard merely printed a message and continued, the alternative type would still be in force and the call would be rejected. The rule rewards the guard-clause arrangement described above: handling absence and failure first is not only clearer, it is what makes the remainder of the function typecheck.

## Pre-Condition Loops: `WHILE`

A `WHILE` loop tests its condition before each iteration, including the first. If the condition is initially `FALSE`, the body never runs:

```basic
LET remaining AS INTEGER = 5
LET sum AS INTEGER = 0

WHILE remaining > 0
    sum += remaining
    remaining--
END WHILE

PRINT sum          // 15
```

Since 0.5.2, `remaining--` and `remaining++` are accepted as statements, exactly equivalent to `remaining -= 1` and `remaining += 1`, with the same typing and the same overflow check. They are statements and not expressions, so `PRINT i++` is a syntax error. The restriction removes the question of whether the increment is applied before or after the surrounding expression is evaluated.

Every `WHILE` loop needs an answer to one question: which quantity mentioned in the condition is changed by the body, and in which direction. In the example the answer is `remaining`, which decreases by one per iteration and is bounded below by the condition. When no such quantity exists the loop does not terminate, and a loop whose termination argument cannot be stated in one sentence should be rewritten rather than tested until it appears to work.

## Post-Condition Loops: `REPEAT ... UNTIL`

A `REPEAT` loop runs its body and then evaluates the condition after `UNTIL`. The loop continues while that condition is `FALSE` and stops when it becomes `TRUE`, which means the body always runs at least once. The block closes with `END REPEAT`:

```basic
FUNCTION Start() AS VOID
    LET answer AS STRING OR EOF = ""
    LET accepted AS BOOLEAN = FALSE

    REPEAT
        INPUT "Continue? (y/n): ", answer
        IF answer IS EOF THEN
            PRINT "input closed"
            RETURN
        END IF
        accepted = (answer = "y") OR (answer = "n")
        IF NOT accepted THEN
            PRINT "Please answer y or n."
        END IF
    UNTIL accepted
    END REPEAT

    PRINT "answer:", answer
END FUNCTION
```

This is the case `REPEAT` exists for: the data that the condition examines does not exist until the body has run once. Written as a `WHILE`, the same logic requires either a duplicated read before the loop or a sentinel value chosen to make the first test fail, and both are ways of simulating the behaviour `REPEAT` provides directly. Note that `UNTIL` states the stopping condition, not the continuation condition; reading it as "repeat until the answer is accepted" gives the correct sense, and translating a `WHILE` condition into an `UNTIL` condition requires negating it.

## Counted Loops: `FOR`

A counted `FOR` loop declares its variable and the variable's integer type in the header, and iterates from the starting value through the ending value inclusive:

```basic
FOR i AS INTEGER = 0 TO 9 STEP 2
    PRINT i          // 0 2 4 6 8
END FOR
```

`STEP` defaults to `1` when omitted and may not be zero; `STEP 0` is rejected at compile time with `TYPE_MISMATCH`, since it describes a loop that cannot progress. A negative step counts downwards and iterates while the variable is greater than or equal to the ending value, so `FOR i AS INTEGER = 5 TO 1 STEP -2` visits 5, 3, and 1. When the range is empty in the direction of travel — `FOR i AS INTEGER = 5 TO 1` with the default step — the body does not run at all, which is the behaviour to rely on when the bound is computed rather than written literally.

To traverse a vector by index, derive the bound from the vector rather than repeating its declared size:

```basic
LET names AS STRING[3] = ["ana", "bruno", "clara"]

FOR index AS INTEGER = 0 TO LEN(names) - 1
    PRINT index, names[index]
END FOR
```

Indices are zero-based, so the last valid index is `LEN(names) - 1`. Writing the bound in terms of `LEN` means that changing the vector's size changes one line instead of two.

Assigning to the loop variable inside the body is accepted by the compiler, and it should still be avoided. The header states how many times the body runs, and an assignment in the body silently contradicts that statement; in the worst case it prevents termination, as in a loop from 1 to 5 whose body sets the counter back to 4 on every pass. When the number of iterations genuinely depends on work done inside the body, that is a `WHILE` loop, and writing it as one puts the controlling condition where a reader will look for it.

## Iterating a Collection: `FOR EACH`

`FOR EACH` binds each element of a fixed-size vector in turn. The element type must match the vector's element type, and the binding is read-only:

```basic
LET names AS STRING[3] = ["ana", "bruno", "clara"]

FOR EACH name AS STRING IN names
    PRINT name
END FOR
```

Assigning to the bound element is rejected:

```
error[TYPE_MISMATCH]: Type mismatch: Expected mutable binding, but found CONST 'name' in assignment target.
```

Prefer `FOR EACH` whenever the position of an element is not part of the computation. It states that every element is visited exactly once, it cannot go out of bounds, and it removes the two places where an indexed loop is usually wrong: the starting index and the final bound. Reserve the counted form for the cases that need the index itself — printing a numbered list, comparing an element with its predecessor, or walking two vectors in step.

## Leaving and Skipping Iterations

Basic Next has no bare `BREAK` or `CONTINUE`. Each form names the kind of loop it controls: `EXIT FOR`, `EXIT WHILE`, and `EXIT REPEAT` leave the loop immediately, while `CONTINUE FOR`, `CONTINUE WHILE`, and `CONTINUE REPEAT` proceed to the next iteration. Naming a kind that does not enclose the statement is a compile-time error:

```
error[INVALID_LOOP_CONTROL]: Invalid loop control: EXIT FOR requires an enclosing FOR loop
```

The rule matters during maintenance. Changing a `WHILE` into a counted `FOR`, or moving a block of statements from one loop into another, turns every affected jump into a diagnostic that names the file and line, instead of leaving a statement that still compiles and now leaves a different loop.

A jump affects the innermost enclosing loop of the named kind. Leaving two nested loops of the same kind therefore takes two statements, and the outer one is normally driven by a result recorded in the inner one:

```basic
LET grid AS INTEGER[3][3] = [[1, 2, 3], [4, 5, 6], [7, 8, 9]]
LET foundRow AS INTEGER = -1

FOR row AS INTEGER = 0 TO 2
    FOR column AS INTEGER = 0 TO 2
        IF grid[row][column] = 5 THEN
            foundRow = row
            EXIT FOR
        END IF
    END FOR
    IF foundRow >= 0 THEN
        EXIT FOR
    END IF
END FOR
```

When this pattern appears, consider extracting the search into a function instead. A function can return the moment it finds what it is looking for, which removes both the flag and the second `EXIT`:

```basic
FUNCTION IndexOf(values AS INTEGER[3], target AS INTEGER) AS INTEGER
    FOR index AS INTEGER = 0 TO LEN(values) - 1
        IF values[index] = target THEN
            RETURN index
        END IF
    END FOR
    RETURN -1
END FUNCTION
```

`CONTINUE` is most useful for filtering: rather than wrapping the body of a loop in `IF isRelevant THEN ... END IF`, skip the irrelevant element at the top and leave the body unindented. The effect on readability is the same as that of a guard clause, applied to an iteration rather than to a function.

## Loops and Return Analysis

The compiler verifies that every execution path of a non-`VOID` function ends in `RETURN` or `STOP`. It does not assume that any loop executes its body, so a `RETURN` placed only inside a loop does not satisfy the rule:

```basic
FUNCTION First(values AS INTEGER[3]) AS INTEGER
    FOR EACH value AS INTEGER IN values
        RETURN value
    END FOR
END FUNCTION
```

```
error[MISSING_RETURN]: Missing return: non-VOID FUNCTION can complete without RETURN expression
```

The requirement is to say what the function yields when the loop finds nothing, which is exactly the case that is easiest to leave undefined. Supplying a value after the loop — a sentinel such as `-1`, or an alternative type such as `INTEGER OR NA` when no sentinel is available — answers the question in the signature rather than leaving it to the caller to discover at runtime.

## Halting the Program

`STOP` terminates the process immediately and returns an exit code to the host. The code is an `INTEGER` between `0` and `255`:

```basic
IF NOT configured THEN
    PRINT "missing configuration"
    STOP 2
END IF
```

`STOP` is for situations in which continuing has no defined meaning. Ordinary termination, including termination that reports failure, belongs in the signature of `Start`: declaring `FUNCTION Start() AS INTEGER` and returning a value passes that value to the host as the exit status, while unwinding through the normal return path. Reserve `STOP` for deeper points in the call stack where returning a status through every intermediate caller would obscure the code that does the work.

## A Worked Example

The following program reads a batch of sensor readings held in a vector, stops at a sentinel value, rejects readings outside the valid range, classifies the rest, and reports a summary. It combines the constructs of this chapter with the practices described alongside them: named predicates, a classification chain ordered from most restrictive to least, filtering with `CONTINUE`, termination with `EXIT`, and a guard clause protecting the final computation from division by zero.

```basic
FUNCTION IsOutOfRange(reading AS INTEGER, upperLimit AS INTEGER) AS BOOLEAN
    RETURN reading < 0 OR reading > upperLimit
END FUNCTION

FUNCTION Band(reading AS INTEGER) AS STRING
    IF reading >= 90 THEN
        RETURN "high"
    ELSE IF reading >= 60 THEN
        RETURN "normal"
    END IF
    RETURN "low"
END FUNCTION

FUNCTION Start() AS VOID
    CONST END_OF_BATCH AS INTEGER = -1
    CONST UPPER_LIMIT AS INTEGER = 100

    LET readings AS INTEGER[8] = [72, 118, 65, 90, -1, 55, 80, 40]
    LET accepted AS INTEGER = 0
    LET total AS INTEGER = 0

    FOR EACH reading AS INTEGER IN readings
        IF reading = END_OF_BATCH THEN
            EXIT FOR
        END IF
        IF IsOutOfRange(reading, UPPER_LIMIT) THEN
            PRINT "rejected:", reading
            CONTINUE FOR
        END IF
        PRINT reading, Band(reading)
        total += reading
        accepted++
    END FOR

    IF accepted = 0 THEN
        PRINT "no usable readings"
        RETURN
    END IF

    PRINT "accepted:", accepted
    PRINT "mean:", total / accepted
END FUNCTION
```

Running it with `bn run readings.bn` produces:

```
72 normal
rejected: 118
65 normal
90 high
accepted: 3
mean: 75.66666666666667
```

Two details are worth noting. The mean is a `FLOAT` because `/` always performs floating-point division, as described in the previous chapter; integer division would require `DIV`. And the guard on `accepted` is not defensive padding: the sentinel may be the first element, in which case no reading is accumulated and the division has no meaning. Writing that case out is cheaper than discovering it from a batch that arrives empty.

## Diagnostics Introduced in This Chapter

| Diagnostic | Raised when |
| --- | --- |
| `TYPE_MISMATCH` | A condition is not `BOOLEAN`; `STEP` is zero; a `FOR EACH` element is assigned. |
| `INVALID_LOOP_CONTROL` | `EXIT` or `CONTINUE` names a loop kind that does not enclose it. |
| `MISSING_RETURN` | A non-`VOID` function has a path that reaches `END FUNCTION` without returning. |
| `E0100` | A single-line `IF` is closed with `END IF`; `++` or `--` is used inside an expression. |

## Summary

Basic Next restricts conditions to `BOOLEAN`, closes every block with an explicit `END`, requires loop jumps to name the loop they control, and checks that every path of a non-`VOID` function returns. Each rule converts a category of runtime defect into a message from `bn check`, and each one is satisfied by writing the intent that was there anyway.

The structural advice is independent of the language and holds wherever these constructs exist. Reject invalid input at the top of a function and leave the main path unindented. Give a condition that expresses a domain rule its own name and its own function. Choose the loop whose shape matches the problem: `WHILE` when the continuation condition is known before the first iteration, `REPEAT` when the data has to be obtained first, `FOR` when the number of iterations is fixed, and `FOR EACH` when the position of an element does not matter. When a loop needs a flag to control a loop outside it, extract the inner loop into a function and return from it instead.

The next chapter introduces the data that these constructs operate on: fixed-size vectors, value types declared with `STRUCT`, and the alternative types used to represent absence and failure.

---

[Next: Compound Data and Error Handling →](04_compound_data.md)
