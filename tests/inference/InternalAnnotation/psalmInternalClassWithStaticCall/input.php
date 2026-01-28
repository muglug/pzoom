<?php
                    namespace A\B {
                        /**
                         * @psalm-internal A
                         */
                        class Foo {
                            public static function barBar(): void {
                            }
                        }
                    }

                    namespace A\B\C {
                        class Bat {
                            public function batBat() : void {
                                \A\B\Foo::barBar();
                            }
                        }
                    }
