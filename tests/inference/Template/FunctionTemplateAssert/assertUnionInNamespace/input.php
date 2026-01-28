<?php
namespace Foo\Bar\Baz;

/**
  * @psalm-template ExpectedType of object
  * @param mixed $value
  * @psalm-param interface-string<ExpectedType> $interface
  * @psalm-assert ExpectedType|interface-string<ExpectedType> $value
  */
function implementsInterface($value, $interface, string $message = ""): void {}

/**
  * @psalm-template ExpectedType of object
  * @param mixed $value
  * @psalm-param interface-string<ExpectedType> $interface
  * @psalm-assert null|ExpectedType|interface-string<ExpectedType> $value
  */
function nullOrImplementsInterface(?object $value, $interface, string $message = ""): void {}

interface A
{
}

/**
 * @param mixed $value
 *
 * @psalm-return A|class-string<A>
 */
function consume($value) {
    implementsInterface($value, A::class);

    return $value;
}

/**
 * @param mixed $value
 *
 * @psalm-return A|class-string<A>|null
 */
function consume2($value)
{
    nullOrImplementsInterface($value, A::class);

    return $value;
}