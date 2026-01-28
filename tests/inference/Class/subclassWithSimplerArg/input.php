<?php
class A {}
class B extends A {}

class E1 {
    /**
     * @param A|B|null $a
     */
    public function __construct($a) {
    }
}

class E2 extends E1 {
    /**
     * @param A|null $a
     */
    public function __construct($a) {
        parent::__construct($a);
    }
}
