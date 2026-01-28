<?php
/**
 * @template TKey1
 * @template TValue1
 */
class Pair
{
    /** @psalm-var TKey1 */
    public $one;

    /** @psalm-var TValue1 */
    public $two;

    /**
     * @psalm-param TKey1 $key
     * @psalm-param TValue1 $value
     */
    public function __construct($key, $value) {
        $this->one = $key;
        $this->two = $value;
    }
}

/**
 * @template TValue2
 * @extends Pair<string, TValue2>
 */
class StringKeyedPair extends Pair {
    /**
     * @param TValue2 $value
     */
    public function __construct(string $key, $value) {
        parent::__construct($key, $value);
    }
}

$pair = new StringKeyedPair("somekey", 250);
$a = $pair->two;
$b = $pair->one;