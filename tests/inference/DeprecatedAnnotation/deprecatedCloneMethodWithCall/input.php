<?php
class Foo {
    /**
     * @deprecated
     */
    public function __clone() {
    }
}

$a = new Foo;
$aa = clone $a;
