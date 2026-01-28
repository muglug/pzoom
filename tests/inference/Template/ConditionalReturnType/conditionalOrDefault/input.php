<?php
/**
 * @template TKey
 * @template TValue
 */
interface C {
    /**
     * @template TDefault
     * @param TKey $key
     * @param TDefault $default
     * @return (
     *     func_num_args() is 1
     *     ? TValue
     *     : TValue|TDefault
     * )
     */
    public function get($key, $default = null);
}

/** @param C<string, DateTime> $c */
function getDateTime(C $c) : DateTime {
    return $c->get("t");
}

/** @param C<string, DateTime> $c */
function getNullableDateTime(C $c) : ?DateTime {
    return $c->get("t", null);
}