<?php
class DiffElem {
    /** @var scalar */
    public $old = false;
    /** @var scalar */
    public $new = false;
}

function foo(DiffElem $diff_elem) : void {
    if ((is_string($diff_elem->old) && is_string($diff_elem->new))
        || (is_int($diff_elem->old) && is_int($diff_elem->new))
    ) {
    }
}