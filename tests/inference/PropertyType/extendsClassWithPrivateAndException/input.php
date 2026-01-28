<?php
abstract class A extends \Exception {
    /** @var string **/
    private $p;

    /** @param string $p **/
    final public function __construct($p) {
        $this->p = $p;
    }
}

final class B extends A {}
