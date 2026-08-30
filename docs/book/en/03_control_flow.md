# Control Flow

Basic Next provides explicit and block-scoped control flow constructs. Every block has a strict opening and closing keyword, such as `END IF` or `END WHILE`.

## Conditional Branching

The `IF` statement evaluates a `BOOLEAN` expression and executes a block of code if the condition is `TRUE`. The condition must be strictly `BOOLEAN`; Basic Next does not implicitly convert integers or strings to boolean values for conditions.

```basic
LET active AS BOOLEAN = TRUE

IF active THEN
    PRINT "System is active."
ELSE
    PRINT "System is offline."
END IF
```

Every `IF` block must be explicitly closed with `END IF`.

## Pre-condition and Post-condition Loops

Basic Next offers two forms of indefinite loops: `WHILE` and `REPEAT`.

### The `WHILE` Loop

A `WHILE` loop checks its `BOOLEAN` condition before executing the block. If the condition is initially `FALSE`, the loop body never executes.

```basic
LET counter AS INTEGER = 0

WHILE counter < 5
    PRINT counter
    counter += 1
END WHILE
```

### The `REPEAT` Loop

A `REPEAT` loop executes its block at least once. It evaluates a `BOOLEAN` post-condition using the `UNTIL` keyword at the end of the block. The block repeats as long as the condition remains `FALSE`.

Note that `UNTIL` is part of the loop's logic, but the block itself must still be closed with `END REPEAT`.

```basic
LET value AS INTEGER = 10

REPEAT
    value -= 1
UNTIL value = 0
END REPEAT
```

## Counted and Collection Iteration

For iterating over ranges or collections, Basic Next provides `FOR` and `FOR EACH`.

### The Counted `FOR` Loop

A counted `FOR` loop iterates a binding over a numeric range. You must declare the loop binding and its type explicitly. The start, end, and optional `STEP` expressions are evaluated exactly once before the loop begins.

```basic
FOR i AS INTEGER = 0 TO 9 STEP 2
    PRINT i
END FOR
```

If you omit the `STEP` clause, it defaults to `1`. A step can be negative, in which case the loop continues while the binding is greater than or equal to the end value. The loop binding updates automatically at the end of the block.

### The `FOR EACH` Loop

`FOR EACH` iterates in index order over a collection. In version 0.2, this is restricted to the outermost dimension of fixed-size vectors. The loop binding is read-only and its declared type must perfectly match the vector's element type.

```basic
LET primes AS INTEGER[3] = [2, 3, 5]

FOR EACH prime AS INTEGER IN primes
    PRINT prime
END FOR
```

## Loop Control and Termination

Basic Next does not have generic `break` or `continue` keywords. Instead, early loop exits must explicitly name the loop type they are targeting: `EXIT FOR`, `EXIT WHILE`, or `EXIT REPEAT`. 

Similarly, skipping to the next iteration uses `CONTINUE FOR`, `CONTINUE WHILE`, or `CONTINUE REPEAT`.

```basic
FOR i AS INTEGER = 1 TO 10
    IF i = 5 THEN
        CONTINUE FOR
    END IF
    IF i = 8 THEN
        EXIT FOR
    END IF
    PRINT i
END FOR
```

By naming the loop construct, you make your intent clear and avoid accidental behavioral changes if loops are refactored or nested differently in the future.

## Halting the Program

If you encounter a fatal condition and must terminate the entire program immediately, use the `STOP` statement.

`STOP` requires a single `INTEGER` expression that produces a value between `0` and `255`. This value is passed directly to the host operating system as the process exit code.

```basic
IF fatalError THEN
    PRINT "Halting immediately."
    STOP 1
END IF
```

For standard, graceful program termination, you should instead `RETURN` an integer from your `Start` function. `STOP` should be reserved for exceptional halting.
