<?php

/** @psalm-immutable */
final class Hasher
{
    public string $hash;
    public string $err;

    /** @param array<string, list<string>> $data */
    public function __construct(array $data)
    {
        // serialize() of any array is impure: Psalm treats arrays as iterable,
        // so canContainObjectType() is true even when no object can appear in
        // the element type.
        $this->hash = serialize($data);
        // json_last_error_msg() takes no arguments, so it reads global state and
        // is impure (Psalm's zero-argument callmap rule).
        $this->err = json_last_error_msg();
    }
}
