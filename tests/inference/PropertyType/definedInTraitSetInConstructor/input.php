<?php
trait A {
    /** @var string **/
    public $a;
}
class B {
    use A;

    public function __construct() {
        $this->a = "hello";
    }
}
