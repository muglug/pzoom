<?php
/** @psalm-no-seal-properties */
class X {
    public function __get(string $name) : string {
        switch ($name) {
            case "a":
                return $this->other;
            case "other":
                return "foo";
        }
        return "default";
    }
}
