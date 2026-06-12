<?php
class C {
    /** @var string */
    public $s;

    public function __construct() {
        $this->setS();
    }

    public function setS() : void {
        $this->s = "hello";
    }
}

class D extends C {
    public function setS() : void {
        // nothing happens here
    }
}
