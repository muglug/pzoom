<?php
class User {
    public string $name = "Dave";
}

/** @return User|NullObject */
function takesNullableUser(?User $user) {
    return optional($user);
}

class NullObject {
    /**
     * @return null
     */
    public function __call(string $_name, array $args) {
        return null;
    }

    /**
     * @return null
     */
    public function __get(string $s) {
        return null;
    }

    public function __set(string $_name, string $_value) : void {
    }
}

/**
 * @template TVar as object|null
 * @param TVar $var
 * @return (TVar is object ? TVar : NullObject)
 */
function optional($var) {
    if ($var) {
        return $var;
    }

    return new NullObject();
}