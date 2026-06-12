<?php

namespace B {
    use A\Consts;

    class User {
        /** @param Consts::VISIBILITY_* $visibility */
        public function check(int $visibility): bool {
            return in_array($visibility, [Consts::VISIBILITY_PUBLIC, Consts::VISIBILITY_PROTECTED], true);
        }

        /** @param list<Consts::VISIBILITY_*> $all */
        public function checkList(array $all): bool {
            $filtered = array_filter($all, static fn(int $v): bool => $v !== Consts::VISIBILITY_PRIVATE);
            return $filtered !== [];
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
