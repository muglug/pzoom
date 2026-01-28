<?php
/**
 * @return array<never, never>|false|string
 */
function foo() {
    return filter_input(INPUT_POST, "some_var") ?? [];
}
