<?php
                    namespace A\B {
                        class Foo {
                            /**
                             * @psalm-internal A\B
                             */
                            public static function barBar(): void {
                            }

                            public static function foo(): void {
                                self::barBar();
                            }
                        }
                    }
