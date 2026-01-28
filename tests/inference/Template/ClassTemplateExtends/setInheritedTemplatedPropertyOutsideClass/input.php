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

/** @extends Watcher<int> */
class IntWatcher extends Watcher {}

$watcher = new IntWatcher(0);
$watcher->value = 10;