<?php
/**
 * @template T
 */
abstract class Stringer {
    /**
     * @param T $t
     */
    public function getString($t, object $o = null) : string {
        return "hello";
    }
}

class A {}

/**
 * @template-extends Stringer<A>
 */
class AStringer extends Stringer {
    public function getString($t, object $o = null) : string {
        if ($o) {
            return parent::getString($o);
        }

        return "a";
    }
}
