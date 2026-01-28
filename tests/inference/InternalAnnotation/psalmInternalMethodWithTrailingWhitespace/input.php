<?php
                    namespace A\B {
                        class Foo {
                            /** @psalm-internal A\B */
                            public static function barBar(): void {
                                self::barBar();
                            }
                        }
                    }
