<?php
trait T {
    public function fooFoo(): void {
    }
}

class B {
    use T {
        fooFoo as barBar;
    }

    public function fooFoo(): void {
        $this->barBar();
    }
}
