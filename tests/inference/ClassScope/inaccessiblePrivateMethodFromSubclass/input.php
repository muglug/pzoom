<?php
class A {
    private function fooFoo(): void {

    }
}

class B extends A {
    public function doFoo(): void {
        $this->fooFoo();
    }
}
