<?php
/** @psalm-suppress MissingParamType */
function foo($data): void {
    /** @psalm-suppress MixedArgument */
    array_unshift($data, $a);
}
