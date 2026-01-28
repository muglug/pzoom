<?php
/**
 * @template TReturnFlag as bool
 * @param mixed $expression
 * @param TReturnFlag $return
 * @psalm-return (TReturnFlag is true ? string : void)
 */
function my_var_export($expression, bool $return = false) {
    if ($return) {
        return var_export($expression, true);
    }

    var_export($expression);
}