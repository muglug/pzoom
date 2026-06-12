<?php
namespace Bar;

/** @psalm-type PhoneType = array{phone: string} */
class Phone {
    /** @psalm-return PhoneType */
    public function toArray(): array {
        return ["phone" => "Nokia"];
    }
}

/** @psalm-type NameType = array{name: string} */
class Name {
    /** @psalm-return NameType */
    function toArray(): array {
        return ["name" => "Matt"];
    }
}

/**
 * @psalm-import-type PhoneType from Phone as PhoneType2
 * @psalm-import-type NameType from Name as NameType2
 *
 * @psalm-type UserType = PhoneType2&NameType2
 */
class User {
    /** @psalm-return UserType */
    function toArray(): array {
        return array_merge(
            (new Name)->toArray(),
            (new Phone)->toArray()
        );
    }
}
