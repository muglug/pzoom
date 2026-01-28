<?php
class A {
    /** @var string */
    public $foo = "bar";

    public function &getString() : string {
        return $this->foo;
    }
}

function useString(string &$s) : void {}
$a = new A();

useString($a->getString());
