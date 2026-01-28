<?php
                    namespace A {
                        /**
                         * @internal
                         */
                        class Foo {
                            public static function barBar(): void {
                            }
                        }
                    }

                    namespace B {
                        class Bat {
                            public function batBat() {
                                \A\Foo::barBar();
                            }
                        }
                    }
