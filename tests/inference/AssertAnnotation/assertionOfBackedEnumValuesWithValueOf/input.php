<?php
enum StringEnum: string
{
    case FOO = "foo";
    case BAR = "bar";
    case BAZ = "baz";
}

enum IntEnum: int
{
    case FOO = 1;
    case BAR = 2;
    case BAZ = 3;
}

/** @psalm-assert value-of<StringEnum::BAR|StringEnum::FOO> $foo */
function assertSomeString(string $foo): void
{}

/** @psalm-assert value-of<IntEnum::BAR|IntEnum::FOO> $foo */
function assertSomeInt(int $foo): void
{}

/** @psalm-assert value-of<StringEnum|IntEnum> $foo */
function assertAnyEnumValue(string|int $foo): void
{}

/** @param "foo"|"bar" $foo */
function takesSomeStringFromEnum(string $foo): StringEnum
{
    return StringEnum::from($foo);
}

/** @param 1|2 $foo */
function takesSomeIntFromEnum(int $foo): IntEnum
{
    return IntEnum::from($foo);
}

/** @var non-empty-string $string */
$string = null;
/** @var positive-int $int */
$int = null;

assertSomeString($string);
takesSomeStringFromEnum($string);

assertSomeInt($int);
takesSomeIntFromEnum($int);

/** @var string|int $potentialEnumValue */
$potentialEnumValue = null;
assertAnyEnumValue($potentialEnumValue);
