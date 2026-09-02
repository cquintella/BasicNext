# Functions and Program Structure

Basic Next programs are composed of functions organized into files called modules. The language requires explicit signatures, strictly verifies return paths, and uses a disciplined import system to prevent namespace pollution.

## Function Declarations

A function is declared using the `FUNCTION` keyword, followed by its parameter list, the `AS` keyword, and its return type. The block is closed with `END FUNCTION`.

```basic
FUNCTION Add(a AS INTEGER, b AS INTEGER) AS INTEGER
    RETURN a + b
END FUNCTION
```

If a function does not return a value, its return type must be explicitly declared as `VOID`. In a `VOID` function, you can omit the `RETURN` statement entirely, or use a bare `RETURN` for an early exit.

```basic
FUNCTION LogMessage(msg AS STRING) AS VOID
    IF msg = "" THEN
        RETURN
    END IF
    PRINT msg
END FUNCTION
```

### Return Analysis

Basic Next employs strict return analysis at compile time. For any function with a return type other than `VOID`, the compiler verifies that *every* possible execution path ends with a valid `RETURN` statement (or a `STOP`).

If a path could theoretically reach `END FUNCTION` without returning a value, the analyzer rejects the program. The analyzer does not assume that loops run forever; a `RETURN` hidden exclusively inside a `WHILE` or `REPEAT` loop does not satisfy the return rule.

## Function Values

Basic Next supports treating functions as first-class values, allowing you to pass them as arguments or store them in variables. The type of a function value is written as `FUNCTION(parameter-types) AS return-type`.

```basic
FUNCTION Double(value AS INTEGER) AS INTEGER
    RETURN value * 2
END FUNCTION

FUNCTION Start() AS VOID
    LET transform AS FUNCTION(INTEGER) AS INTEGER = Double
    LET result AS INTEGER = transform(21)
    PRINT result
END FUNCTION
```

In version 0.3, only module-level functions and `STATIC` class methods can be used as function values. Instance methods cannot be stored this way, and there are no lambdas or closures.

## Modules and Visibility

Every `.bn` source file is a module. By default, any `FUNCTION`, `CLASS`, `STRUCT`, or `INTERFACE` declared in a module is private to that module. 

To make a declaration visible to other files, you must precede it with the `EXPORT` keyword:

```basic
// In MathUtils.bn
EXPORT FUNCTION Square(n AS INTEGER) AS INTEGER
    RETURN n * n
END FUNCTION

FUNCTION Helper() AS VOID
    // Only visible inside MathUtils.bn
END FUNCTION
```

## Importing Modules

To use exported declarations from another module, you must import it using the `IMPORT` keyword. Every import requires an explicit local alias using `AS`. 

```basic
// In main.bn
IMPORT MathUtils AS Math

FUNCTION Start() AS VOID
    PRINT Math.Square(5)
END FUNCTION
```

Imported members are only accessible through their alias (e.g., `Math.Square`). Basic Next never injects imported names into your module's global scope, ensuring that two modules can export functions with the same name without colliding.

Module resolution happens relative to the project root (the directory of the executable module), typically under a `modules/` directory. Language standard-library modules resolve from `modules/bn/`. For example, `IMPORT BNData AS Data` resolves to `modules/bn/BNData.bn`. Host capabilities use the `HOST` root, such as `IMPORT HOST.Random AS R`. `BNMath` is a standard-library module and requires `IMPORT BNMath AS Math`.

Basic Next checks for import cycles and will reject the program if a circular dependency is detected.
