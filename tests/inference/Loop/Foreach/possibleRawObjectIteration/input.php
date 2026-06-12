<?php
class A {
    /** @var ?string */
    public $foo;
}

class B extends A {}

function bar(A $a): void {}

$arr = [];

if (rand(0, 10) > 5) {
    $arr[] = new A;
} else {
    $arr = new B;
}

foreach ($arr as $a) {
    bar($a);
}
