<?php
    final class C {
        /** @var null|int */
        private static $q = null;

        /** @psalm-assert int self::$q */
        private static function prefillQ(): void {
            self::$q = 123;
        }

        public static function getQ(): int {
            self::prefillQ();
            return self::$q;
        }
    }
?>
