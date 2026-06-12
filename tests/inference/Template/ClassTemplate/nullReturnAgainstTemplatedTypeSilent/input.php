<?php

/** @template T as array|object|string */
class Cache9 {
    /** @var T|null */
    private $item = null;

    /** @return T */
    public function getItem(bool $flag): array|object|string|null
    {
        if ($flag) {
            return null;
        }
        return $this->item;
    }
}
