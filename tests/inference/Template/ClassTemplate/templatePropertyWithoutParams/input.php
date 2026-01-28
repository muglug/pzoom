<?php
/**
 * @template T of object
 */
class Batch {
    /**
     * @var iterable<T>
     */
    public iterable $objects = [];

    /**
     * @var callable(T): void
     */
    public $onEach;

    public function __construct() {
        $this->onEach = function (): void {};
    }
}

function handle(Batch $message, object $o): void {
    $fn = $message->onEach;
    $fn($o);
}