<?php
/** @template T */
final class a {
    /** @param T $t */
    public function __construct(public mixed $t) {}
}

$a = get_object_vars(new a("test"));
