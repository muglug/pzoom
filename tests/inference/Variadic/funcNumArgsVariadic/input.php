<?php
function test(): array {
    return func_get_args();
}
var_export(test(2));
