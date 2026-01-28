<?php
class ParentClass {
    public function __call(string $name, array $args) {}
}

/**
 * @method string getString() dsa sada
 * @method  void setInteger(int $integer) dsa sada
 * @method setString(int $integer) dsa sada
 * @method setMixed(mixed $foo) dsa sada
 * @method setImplicitMixed($foo) dsa sada
 * @method setAnotherImplicitMixed( $foo, $bar,$baz) dsa sada
 * @method setYetAnotherImplicitMixed( $foo  ,$bar,  $baz    ) dsa sada
 * @method  getBool(string $foo)  :   bool dsa sada
 * @method (string|int)[] getArray() with some text dsa sada
 * @method (callable() : string) getCallable() dsa sada
 */
class Child extends ParentClass {}

$child = new Child();

$a = $child->getString();
$child->setInteger(4);
/** @psalm-suppress MixedAssignment */
$b = $child->setString(5);
$c = $child->getBool("hello");
$d = $child->getArray();
$e = $child->getCallable();
$child->setMixed("hello");
$child->setMixed(4);
$child->setImplicitMixed("hello");
$child->setImplicitMixed(4);
