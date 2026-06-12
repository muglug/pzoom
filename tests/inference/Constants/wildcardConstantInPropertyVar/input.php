<?php

namespace B {
    use A\Consts;

    class Storage {
        /** @var Consts::VISIBILITY_* */
        public int $visibility = Consts::VISIBILITY_PUBLIC;
    }

    class User {
        /** @param list<Consts::VISIBILITY_*> $allowed */
        public function check(Storage $s, array $allowed): bool {
            return in_array($s->visibility, $allowed);
        }
    }
}

namespace A {
    class Consts {
        public const VISIBILITY_PUBLIC = 1;
        public const VISIBILITY_PROTECTED = 2;
        public const VISIBILITY_PRIVATE = 3;
    }
}
