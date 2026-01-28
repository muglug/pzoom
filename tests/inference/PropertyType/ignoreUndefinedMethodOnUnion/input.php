<?php
/** @psalm-no-seal-properties */
class NullObject {
    /**
     * @return null
     */
    public function __get(string $s) {
        return null;
    }
}

class User {
    public string $name = "Dave";
}

function takesNullableUser(User|NullObject $user) : ?string {
    $name = $user->name;

    if ($name === null) {}

    return $name;
}
