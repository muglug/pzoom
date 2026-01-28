<?php

/** @template T as scalar */
class Collection {
    /** @var T */
    public $val;

    /** @param T $val */
    public function __construct($val) {
        $this->val = $val;
    }

    public function foo() : string {
        if ($this instanceof StringCollection) {
            return $this->val;
        }

        return "hello";
    }
}

/** @extends Collection<string> */
class StringCollection extends Collection {}