<?php
/** @param array<int|string, string> $args */
function f(array $args): void {
    /** @var string $arg_name */
    foreach ($args as $arg_name => $arg_type) {
        if (str_ends_with($arg_name, '=')) {
            echo $arg_type;
        }
    }
}
