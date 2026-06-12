<?php
namespace Barrr;

class Phone {
    function toArray(): array {
        return ["name" => "Matt"];
    }
}

/**
 * @psalm-import-type PhoneType from Phone
 */
class User {}
