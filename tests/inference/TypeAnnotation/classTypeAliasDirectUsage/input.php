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
 * @psalm-import-type PhoneType from Phone
 */
class User {
    /** @psalm-return PhoneType */
    function toArray(): array {
        return array_merge([], (new Phone)->toArray());
    }
}
