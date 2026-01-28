<?php

/**
 * @template TTKey of array-key
 * @template TTValue
 *
 * @extends ArrayObject<TTKey, TTValue>
 */
class iter extends ArrayObject {
    /**
     * @return self<TTKey, TTValue>
     */
    public function stabilize(): self {
        return $this;
    }
}

class a {
    /**
     * @return iter<int, static>
     */
    public function ret(): iter {
        return new iter([$this]);
    }
}
class b extends a {
}

$a = new b;
$a = $a->ret();
$a = $a->stabilize();
