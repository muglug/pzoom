<?php
trait T3 {
    abstract public function test(int $x) : void;
}

class P3 {
    public function test(string $x) : void {}
}

class C3 extends P3 {
    use T3;
}
