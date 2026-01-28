<?php
/**
 * @psalm-consistent-constructor
 */
class C {
    public function __construct() {}
}

class C2 extends C {
    public function __construct() {}
}
