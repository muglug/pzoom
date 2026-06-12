<?php
namespace Foo {
    /** @psalm-type PhoneType = array{phone: string} */
    class Phone {
        /** @psalm-return PhoneType */
        public function toArray(): array {
            return ["phone" => "Nokia"];
        }
    }
}

namespace Bar {
    /**
     * @psalm-import-type PhoneType from \Foo\Phone
     */
    class User {
        /** @psalm-return PhoneType */
        function toArray(): array {
            return (new \Foo\Phone)->toArray();
        }
    }
}
