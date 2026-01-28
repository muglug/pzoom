<?php
class A {
    /** @var A|null */
    public $foo;

    public function __toString(): string {
        return "boop";
    }
}

$a = rand(0, 10) === 5 ? new A() : null;

if (rand(0, 1)) {

} elseif ($a && $a->foo) {
    echo $a;
}