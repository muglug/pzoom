<?php
interface I {
    public function foo(): void;
}
abstract class A {
    /** @var string */
    public $name = "";
    abstract public function bar(): void;
}
class C extends A implements I {
    public function foo(): void {}
    public function bar(): void {}
}

/** @var (A&I)[] $instances */
$instances = [];
foreach (array_column($instances, null, "name") as $instance) {
    $instance->foo();
    $instance->bar();
}
            
