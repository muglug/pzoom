<?php
/** @template T of 'idle'|'running' */
class App {
    /** @psalm-this-out self<'idle'> */
    public function __construct() {}

    /**
     * @psalm-if-this-is self<'idle'>
     * @psalm-this-out self<'running'>
     */
    public function start(): void {}
}
$app = new App();
