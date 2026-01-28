<?php
trait T {
    protected function fooFoo(): void {
    }
}

class B {
    use T;
}

class C {
    use T;

    public function doFoo(): void {
        (new B)->fooFoo();
    }
}
