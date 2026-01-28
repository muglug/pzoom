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

/** @psalm-var Watcher<int> $watcher */
$watcher = new Watcher(0);
$watcher->value = 0;