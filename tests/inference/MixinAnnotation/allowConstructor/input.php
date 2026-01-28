<?php
abstract class AParent {
    protected int $i;

    public function __construct() {
        $this->i = 1;
    }
}

class M {
    public function __construct() {}
}

/**
 * @mixin M
 */
class A extends AParent {}
