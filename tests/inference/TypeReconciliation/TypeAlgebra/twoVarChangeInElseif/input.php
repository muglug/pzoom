<?php
class A {
    public function takesA(A $a) : void {}

    public function foo() : void {}
}

function formatRange(?A $from, ?A $to): void {
    if (!$to && !$from) {
        $to = new A();
        $from = new A();
    } elseif (!$from) {
        $from = new A();
        $from->takesA($to);
    } elseif (!$to) {
        $to = new A();
        $to->takesA($from);
    }

    $from->foo();
    $to->foo();
}