<?php
class Base {
    /** @var string */
    protected $prop;
    public function __construct(string $s) {
        $this->prop = $s;
    }
}

class Child extends Base {
    /** @var string */
    protected $prop;
}
