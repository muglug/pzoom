<?php
class A {
    /**
     * @psalm-suppress MixedMethodCall
     */
    public function foo(object $a) : void {
        $a->bar("bat" . $this->baz);
    }
}
