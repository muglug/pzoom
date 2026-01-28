<?php
/**
 * @template TValue as scalar
 */
class Watcher {
    /**
     * @psalm-var TValue
     */
    public $value;

    /**
     * @psalm-param TValue $value
     */
    public function __construct($value) {
        $this->value = $value;
    }
}

/**
 * @template T as scalar
 * @extends Watcher<T>
 */
class Watcher2 extends Watcher {}

/** @psalm-var Watcher2<int> $watcher */
$watcher = new Watcher2(0);
$watcher->value = 10;