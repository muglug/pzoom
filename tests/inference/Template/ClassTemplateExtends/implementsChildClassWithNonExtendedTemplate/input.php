<?php
/**
 * @template T
 */
class Base {
    /** @var T */
    private $t;

    /** @param T $t */
    public function __construct($t) {
        $this->t = $t;
    }

    /**
     * @param T $x
     * @return T
     */
    function example($x) {
        return $x;
    }
}

class Child extends Base {
    function example($x) {
        return $x;
    }
}

/** @param Child $c */
function bar(Child $c) : void {
    ord($c->example("boris"));
}
