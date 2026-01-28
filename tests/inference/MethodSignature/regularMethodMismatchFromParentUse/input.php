<?php
trait T2 {
    abstract public function test(int $x) : void;
}

abstract class P2 {
    use T2;
}

class C2 extends P2 {
    public function test(string $x) : void {}
}
