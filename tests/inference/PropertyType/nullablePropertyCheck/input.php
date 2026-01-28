<?php
class A {
    /** @var string */
    public $aa = "";
}

class B {
    /** @var A|null */
    public $bb;
}

$b = rand(0, 10) ? new A() : new B();

if ($b instanceof B && isset($b->bb) && $b->bb->aa === "aa") {
    echo $b->bb->aa;
}
