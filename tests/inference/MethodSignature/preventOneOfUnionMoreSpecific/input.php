<?php
class A {
    /** @param string|int $s */
    public function foo($s) : void {}
}

class B extends A {
    /** @param string $s */
    public function foo($s) : void {}
}
