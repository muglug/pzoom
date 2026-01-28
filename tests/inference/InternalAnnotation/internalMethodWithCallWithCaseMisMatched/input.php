<?php
                    namespace A\B {
                        class Foo {
                            /**
                             * @psalm-internal A\B
                             */
                            public static function barBar(): void {
                            }
                        }
                    }

                    namespace a\b\c {
                        class Bat {
                            public function batBat() : void {
                                \A\B\Foo::barBar();
                            }
                        }
                    }
