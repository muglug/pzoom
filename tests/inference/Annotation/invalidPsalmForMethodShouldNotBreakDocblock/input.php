<?php
class A {
    /**
     * @psalm-impure
     * @param string $arg
     * @return non-falsy-string
     */
    public function foo($arg) {
        return $arg . "bar";
    }
}

$a = new A();
$_ = $a->foo("hello");
