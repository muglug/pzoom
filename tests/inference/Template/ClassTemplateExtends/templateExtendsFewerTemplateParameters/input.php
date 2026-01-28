<?php
class Real {}

class RealE extends Real {}

/**
 * @template TKey as array-key
 * @template TValue as object
 */
class a {
    /**
     * @param TKey $key
     * @param TValue $real
     */
    public function __construct(public int|string $key, public object $real) {}
    /**
     * @return TValue
     */
    public function ret(): object {
        return $this->real;
    }
}
/**
 * @template TTKey as array-key
 * @template TTValue as object
 *
 * @extends a<TTKey, TTValue>
 */
class b extends a {
}

/**
 * @template TObject as Real
 *
 * @extends b<string, TObject>
 */
class c1 extends b {
    /**
     * @param TObject $real
     */
    public function __construct(object $real) {
        parent::__construct("", $real);
    }
}

/**
 * @template TObject as Real
 * @template TOther
 *
 * @extends b<string, TObject>
 */
class c2 extends b {
    /**
     * @param TOther $other
     * @param TObject $real
     */
    public function __construct($other, object $real) {
        parent::__construct("", $real);
    }
}

/**
 * @template TOther as object
 * @template TObject as Real
 *
 * @extends b<string, TObject|TOther>
 */
class c3 extends b {
    /**
     * @param TOther $other
     * @param TObject $real
     */
    public function __construct(object $other, object $real) {
        parent::__construct("", $real);
    }
}

$a = new a(123, new RealE);
$resultA = $a->ret();

$b = new b(123, new RealE);
$resultB = $b->ret();

$c1 = new c1(new RealE);
$resultC1 = $c1->ret();

$c2 = new c2(false, new RealE);
$resultC2 = $c2->ret();


class Secondary {}

$c3 = new c3(new Secondary, new RealE);
$resultC3 = $c3->ret();
                