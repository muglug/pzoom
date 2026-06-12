<?php
namespace Bar;

/** @psalm-type PhoneType = array{phone: string} */
class Phone {
    /** @psalm-return PhoneType */
    public function toArray(): array {
        return ["phone" => "Nokia"];
    }
}

/**
 * @psalm-import-type PhoneType from Phone as TPhone
 */
class User {
    /** @psalm-return TPhone */
    function toArray(): array {
        return array_merge([], (new Phone)->toArray());
    }
}
