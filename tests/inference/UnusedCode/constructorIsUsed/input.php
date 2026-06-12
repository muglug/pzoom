<?php
final class A {
    public function __construct() {
        $this->foo();
    }
    private function foo() : void {}
}
$a = new A();
echo (bool) $a;
