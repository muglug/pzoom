<?php
interface Input {}
interface HasFoo {}
interface HasBar {}

/**
 * @psalm-template InputType of Input
 * @psalm-param InputType $input
 * @psalm-return InputType&HasFoo
 */
function decorateWithFoo(Input $input): Input
{
    assert($input instanceof HasFoo);
    return $input;
}

/**
 * @psalm-template InputType of Input
 * @psalm-param InputType $input
 * @psalm-return InputType&HasBar
 */
function decorateWithBar(Input $input): Input
{
    assert($input instanceof HasBar);
    return $input;
}

/** @param HasFoo&HasBar $input */
function useFooAndBar(object $input): void {}

function consume(Input $input): void {
    useFooAndBar(decorateWithFoo(decorateWithBar($input)));
}