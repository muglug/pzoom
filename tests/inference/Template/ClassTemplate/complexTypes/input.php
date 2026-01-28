<?php

/**
 * @template T
 */
class Future {
    /**
     * @param T $v
     */
    public function __construct(private $v) {}
    /** @return T */
    public function get() { return $this->v; }
}


/**
 * @template TTObject
 *
 * @extends Future<ArrayObject<int, TTObject>>
 */
class FutureB extends Future {
    /** @param TTObject $data */
    public function __construct($data) { parent::__construct(new ArrayObject([$data])); }
}

$a = new FutureB(123);

$r = $a->get();
