# Object-Oriented Features

While Basic Next provides `STRUCT` for simple value types, it uses `CLASS` and `INTERFACE` for reference types, encapsulation, and polymorphism. 

## Reference Types (`CLASS`)

A `CLASS` defines a reference type. Unlike a struct, assigning a class instance to a new variable does not copy the underlying data; it copies the reference. Both variables will point to the same object in memory.

Class instances are always allocated dynamically using the `NEW` keyword.

```basic
LET customer AS Customer = NEW Customer(10)
```

*(Note: The explicit lifecycle of class instances, including ARC and the `RELEASE` statement, is covered in detail in Chapter 7: Memory Management).*

## Visibility and `SELF`

By default, all fields and methods within a class are `PRIVATE`. Private members are scoped to the declaring class, meaning any method of that class can access the private members of any instance of that same class.

To make a member available to external code, you must explicitly mark it as `PUBLIC`.

A third level, `PROTECTED` (0.5.2), makes a member visible to the declaring class and to classes that `EXTENDS` it — including subclasses in other modules — and to nothing else; reaching it from outside is `PROTECTED_ACCESS`.

Inside a class method, you cannot access instance fields or methods using an unqualified name. You must always explicitly qualify instance access using the `SELF` keyword.

```basic
CLASS Counter
    PRIVATE count AS INTEGER = 0

    PUBLIC FUNCTION Increment() AS VOID
        SELF.count += 1
    END FUNCTION
END CLASS
```

## Constructors and Destructors

A class can define exactly one constructor to initialize its state. Overloading is not supported in version 0.3.

The constructor is declared as `FUNCTION CONSTRUCTOR` with an optional parameter list. It does not have a return type and is invoked automatically when `NEW` is called. If you do not declare a constructor, the compiler provides an implicit, parameterless `PRIVATE` constructor.

```basic
CLASS Customer
    PRIVATE id AS INTEGER

    PUBLIC FUNCTION CONSTRUCTOR(id AS INTEGER)
        SELF.id = id
    END FUNCTION
END CLASS
```

You may also define a destructor using `FUNCTION DESTRUCTOR()`. The destructor takes no parameters and has no return type. It executes exactly once when the last strong reference is released, either by scope exit or `RELEASE`.

## Static Members

Basic Next supports class-level state and behavior through the `STATIC` keyword. A static field exists exactly once per class. If the initializer is omitted, a defaultable type uses the same default as `LET` (`INTEGER` is `0`, `STRING` is `""`, `BOOLEAN` is `FALSE`). Types without a default (`POINTER`, class-typed fields, alternatives) still require `=`. A static method cannot use the `SELF` keyword or access instance fields.

Static members are always accessed through the class name, never through an instance.

```basic
CLASS Session
    PRIVATE STATIC nextId AS INTEGER = 0

    PUBLIC STATIC FUNCTION NextId() AS INTEGER
        Session.nextId += 1
        RETURN Session.nextId
    END FUNCTION
END CLASS

// Accessing the static method
LET id AS INTEGER = Session.NextId()
```

Re-entering the initialization of static fields raises a `STATIC_INITIALIZATION_CYCLE` error at runtime, ensuring partially initialized state is never observable.

Multiple ways to construct a class are expressed with `PUBLIC STATIC FUNCTION` factories that return `NEW` (for example `Temperature.FromFahrenheit(f)`, see `examples/temperature-factories.bn`); a second `CONSTRUCTOR` signature is a static error.

## Inheritance

Basic Next supports single class inheritance using the `EXTENDS` keyword. A subclass inherits the methods and fields of its base class. 

If the base class has a constructor, the subclass constructor must call it as the first statement using the `SUPER` keyword.

```basic
CLASS Animal
    PUBLIC FUNCTION Speak() AS VOID
        PRINT "..."
    END FUNCTION
END CLASS

CLASS Dog EXTENDS Animal
    PUBLIC FUNCTION CONSTRUCTOR()
        SUPER()
    END FUNCTION

    PUBLIC OVERRIDE FUNCTION Speak() AS VOID
        PRINT "Woof"
    END FUNCTION
END CLASS
```

Methods in the subclass automatically override methods in the base class with the same signature. Virtual dispatch ensures the correct method is called at runtime, even when the object is accessed through a base class reference (upcast).

```basic
LET myDog AS Dog = NEW Dog()
LET myAnimal AS Animal = myDog // Upcast
myAnimal.Speak() // Prints "Woof"
```

Since 0.5.2 a method that overrides an inherited one must carry `OVERRIDE`, written `PUBLIC OVERRIDE FUNCTION Speak() AS STRING`: `bn check` reports `OVERRIDE_REQUIRED` when the marker is missing and `OVERRIDE_WITHOUT_BASE` when no ancestor declares an overridable method. `STATIC` methods are not virtual. Narrowing a base-typed reference back to a subclass (`pet AS Dog` → `Dog OR Error`) is specified but not yet available.

## Contracts (`INTERFACE` and `IMPLEMENTS`)

An `INTERFACE` is a named public contract consisting only of function signatures. It cannot contain fields, constructors, or implementation bodies. Interface members are implicitly public.

```basic
INTERFACE Printable
    FUNCTION Print() AS VOID
END INTERFACE
```

A class implements one or more interfaces explicitly using the `IMPLEMENTS` keyword followed by a comma-separated list of interface names (which can be imported from other modules, e.g., `IMPLEMENTS Pets.Named`). The class must provide a `PUBLIC` instance method for every required signature, matching the parameter count, types, and return type perfectly.

```basic
CLASS Report IMPLEMENTS Printable
    PUBLIC FUNCTION Print() AS VOID
        PRINT "Report data"
    END FUNCTION
END CLASS
```

An interface name acts as a type. A class reference can be assigned to a variable typed as an interface it implements. This implicit upcast preserves the object reference but restricts access to only the interface's members. In version 0.3, you cannot downcast an interface value back to a concrete class.
