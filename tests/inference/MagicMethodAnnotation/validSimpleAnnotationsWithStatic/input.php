<?php
class ParentClass {
    public function __callStatic(string $name, array $args) {}
}

/**
 * @method static string getString() dsa sada
 * @method static void setInteger(int $integer) dsa sada
 * @method static mixed setString(int $integer) dsa sada
 * @method static mixed setMixed(mixed $foo) dsa sada
 * @method static mixed setImplicitMixed($foo) dsa sada
 * @method static mixed setAnotherImplicitMixed( $foo, $bar,$baz) dsa sada
 * @method static mixed setYetAnotherImplicitMixed( $foo  ,$bar,  $baz    ) dsa sada
 * @method static bool getBool(string $foo)   dsa sada
 * @method static (string|int)[] getArray() with some text dsa sada
 * @method static (callable() : string) getCallable() dsa sada
 * @method static static getInstance() dsa sada
 */
class Child extends ParentClass {}

$a = Child::getString();
Child::setInteger(4);
/** @psalm-suppress MixedAssignment */
$b = Child::setString(5);
$c = Child::getBool("hello");
$d = Child::getArray();
$e = Child::getCallable();
$f = Child::getInstance();
Child::setMixed("hello");
Child::setMixed(4);
Child::setImplicitMixed("hello");
Child::setImplicitMixed(4);
