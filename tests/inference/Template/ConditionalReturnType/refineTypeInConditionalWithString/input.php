<?php
/**
 * @template TInput
 *
 * @param TInput $input
 *
 * @return (TInput is string ? TInput : 'hello')
 */
function foobaz($input): string {
    if (is_string($input)) {
        return $input;
    }

    return "hello";
}

$a = foobaz("boop");
$b = foobaz(4);