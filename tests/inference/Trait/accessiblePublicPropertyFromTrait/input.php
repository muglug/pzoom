<?php
trait T {
    /** @var string */
    public $fooFoo = "";
}

class B {
    use T;

    public function doFoo(): void {
        echo $this->fooFoo;
        $this->fooFoo = "hello";
    }
}
