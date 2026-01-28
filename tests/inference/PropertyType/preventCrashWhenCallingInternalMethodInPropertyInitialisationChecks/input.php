<?php
class Foo extends \RuntimeException {
    /** @var array */
    protected $serializableTrace;

    public function __construct() {
        parent::__construct("hello", 0);
        $this->serializableTrace = $this->getTrace();
    }
}

class Bar extends Foo {}
