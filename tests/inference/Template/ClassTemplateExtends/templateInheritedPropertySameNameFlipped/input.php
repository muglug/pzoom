<?php
/**
 * @template TKey
 * @template TValue
 */
class Pair
{
    /** @psalm-var TKey */
    public $one;

    /** @psalm-var TValue */
    public $two;

    /**
     * @psalm-param TKey $key
     * @psalm-param TValue $value
     */
    public function __construct($key, $value) {
        $this->one = $key;
        $this->two = $value;
    }
}

/**
 * @template TValue
 * @extends Pair<TValue, string>
 */
class StringKeyedPair extends Pair {
    /**
     * @param TValue $value
     */
    public function __construct(string $key, $value) {
        parent::__construct($value, $key);
    }
}

$pair = new StringKeyedPair("somekey", 250);
$a = $pair->one;
$b = $pair->two;