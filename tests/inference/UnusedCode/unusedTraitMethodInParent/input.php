<?php
class A {
    public function foo() : void {}
}
trait T {
    public function foo() : void {}

    public function bar() : void {}
}
final class B extends A {
    use T;
}
function takesA(A $a) : void {
    $a->foo();
}
takesA(new B);
