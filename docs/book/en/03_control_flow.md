# Control Flow

Programs make decisions and repeat actions based on conditions and runtime data. Basic Next provides structured, block-scoped control flow statements with explicit openings and closings.

## Conditional Branching

The `IF` statement evaluates a `BOOLEAN` condition and executes a body of statements when the condition is `TRUE`. Conditions must strictly evaluate to `BOOLEAN`; integers or strings are never treated as truthy or falsy in conditions.

### Block `IF` Statements

Standard conditionals use the block form, terminated by `END IF`:

```basic
LET active AS BOOLEAN = TRUE

IF active THEN
    PRINT "System is active."
ELSE
    PRINT "System is offline."
END IF
```

You can chain additional conditions using `ELSE IF`:

```basic
IF score >= 90 THEN
    PRINT "Grade: A"
ELSE IF score >= 80 THEN
    PRINT "Grade: B"
ELSE
    PRINT "Grade: C"
END IF
```

### Single-Line `IF` Statements

When a conditional consists of a single simple statement per branch, Basic Next allows a concise single-line form:

```basic
IF condition THEN simple-statement [ELSE simple-statement]
```

Single-line `IF` statements must fit entirely on one physical line and do not use `END IF`:

```basic
LET flag AS BOOLEAN = TRUE

IF flag THEN PRINT "yes" ELSE PRINT "no"
IF count > 100 THEN STOP 1
```

## Indefinite Loops

Basic Next provides two loop constructs for conditions evaluated before or after iterations: `WHILE` and `REPEAT`.

### The `WHILE` Loop

A `WHILE` loop tests its condition before executing the body. If the condition is initially `FALSE`, the loop body is skipped entirely:

```basic
LET counter AS INTEGER = 0

WHILE counter < 5
    PRINT counter
    counter += 1
END WHILE
```

### The `REPEAT` Loop

A `REPEAT` loop executes its body at least once. It evaluates a post-condition with `UNTIL`, repeating as long as the condition remains `FALSE`:

```basic
LET value AS INTEGER = 10

REPEAT
    value -= 1
UNTIL value = 0
END REPEAT
```

The loop block must always close with `END REPEAT`.

## Counted and Collection Iteration

Basic Next provides `FOR` and `FOR EACH` for fixed counts and collection traversal.

### The Counted `FOR` Loop

A counted `FOR` loop increments a numeric variable across a specified range. The loop variable and its type are declared directly in the header:

```basic
FOR i AS INTEGER = 0 TO 9 STEP 2
    PRINT i
END FOR
```

If `STEP` is omitted, it defaults to `1`. Negative step values iterate downwards as long as the counter is greater than or equal to the target value. The block closes with `END FOR`.

### The `FOR EACH` Loop

`FOR EACH` iterates over elements of a fixed-size vector. The loop variable is read-only and its type must match the vector's element type:

```basic
LET primes AS INTEGER[3] = [2, 3, 5]

FOR EACH prime AS INTEGER IN primes
    PRINT prime
END FOR
```

## Loop Control and Termination

Basic Next avoids ambiguous generic break statements. Loop exits and jumps must explicitly state the loop kind being controlled:

- `EXIT FOR`, `EXIT WHILE`, or `EXIT REPEAT` exits the loop immediately.
- `CONTINUE FOR`, `CONTINUE WHILE`, or `CONTINUE REPEAT` jumps directly to the next iteration.

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

Explicitly specifying the loop construct prevents accidental bugs during refactoring and nested loop maintenance.

## Halting the Program

To terminate program execution immediately upon an unrecoverable error, use the `STOP` statement.

`STOP` takes an `INTEGER` value between `0` and `255`, which is returned to the host operating system as the exit code:

```basic
IF fatalError THEN
    PRINT "Halting immediately."
    STOP 1
END IF
```

For routine program termination, return a status code from your `Start` function. `STOP` is intended for abnormal, unrecoverable situations.
