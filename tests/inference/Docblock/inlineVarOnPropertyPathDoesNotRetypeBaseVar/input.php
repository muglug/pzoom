<?php
class Ctx {
    /** @var array<string, int> */
    public array $map = [];
    /** @var array<string, list<string>> */
    public array $thrown = [];
}
/** @param array<string, int> $m */
function takesArray(array $m): void {}

function f(Ctx $context): void {
    /**
     * @var array<string, list<string>> $context->thrown
     */
    $context->thrown = [];

    $try_context = clone $context;
    takesArray($try_context->map);
}
