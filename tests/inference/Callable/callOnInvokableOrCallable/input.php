<?php
interface Callback {
    public function __invoke(): void;
}

/** @var Callback|callable */
$test = function (): void {};

$test();
