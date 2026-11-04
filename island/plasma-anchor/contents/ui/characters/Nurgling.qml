import QtQuick
import QtQuick.Shapes

// Original miniature from the official Nurglings reference; see revision verification.
HabitCharacter {
    id: art
    Shape {
        width: 20; height: 20
        preferredRendererType: Shape.CurveRenderer
        ShapePath {
            fillColor: "#ded2a2"; strokeColor: "#756341"; strokeWidth: 0.7
            PathSvg { path: "M9 5 Q6 3 8 0 L9 0 Q7 2 12 3 L12 5 Z" }
        }
    }
    Shape {
        width: 20; height: 20
        preferredRendererType: Shape.CurveRenderer
        ShapePath {
            fillColor: "#788345"; strokeColor: "#343c25"; strokeWidth: 0.8
            PathSvg { path: "M5 15 L4 19 8 19 9 16 M12 16 L13 19 17 19 15 15 Z" }
        }
    }
    Shape {
        width: 20; height: 20
        preferredRendererType: Shape.CurveRenderer
        ShapePath {
            fillColor: "#899449"; strokeColor: "#343c25"; strokeWidth: 0.8
            PathSvg { path: "M5 9 Q2 8 1 12 L1 15 3 14 4 12 6 12 Z" }
        }
    }
    Shape {
        width: 20; height: 20
        preferredRendererType: Shape.CurveRenderer
        ShapePath {
            fillColor: "#929d50"; strokeColor: "#343c25"; strokeWidth: 0.8
            PathSvg { path: "M6 4 Q9 2 13 4 L15 7 14 9 Q18 12 16 16 Q15 19 9 18 Q4 18 4 14 Q3 11 6 9 Q3 6 6 4 Z" }
        }
    }
    Shape {
        width: 20; height: 20
        preferredRendererType: Shape.CurveRenderer
        ShapePath {
            fillColor: "#b3b96a"; strokeColor: "transparent"; strokeWidth: 0
            PathSvg { path: "M7 11 Q12 10 15 13 Q17 17 11 17 Q6 18 5 14 Z" }
        }
    }
    Item {
        objectName: "scratchArm"
        y: art.happy ? -2 : -art.snap(art.step * art.pixel)
    Shape {
        width: 20; height: 20
        preferredRendererType: Shape.CurveRenderer
        ShapePath {
            fillColor: "#8b984b"; strokeColor: "#343c25"; strokeWidth: 0.8
            PathSvg { path: "M14 10 Q17 8 18 10 L19 14 17 15 16 12 14 13 Z" }
        }
    }
    Rectangle { x: 17; y: 13; width: 1; height: 2; radius: 0; color: "#d2c99c"; antialiasing: true;  }
    Rectangle { x: 18; y: 13; width: 1; height: 1; radius: 0; color: "#d2c99c"; antialiasing: true;  }
    }
    Rectangle { x: 6 + art.face.shownX; y: 5 + art.face.shownY; width: 1; height: Math.max(0.5, art.face.blink * art.face.lid); radius: 0.6; color: "#242619"; antialiasing: true; objectName: "companionEye0" }
    Rectangle { x: 11 + art.face.shownX; y: 5 + art.face.shownY; width: 1; height: Math.max(0.5, art.face.blink * art.face.lid); radius: 0.6; color: "#242619"; antialiasing: true; objectName: "companionEye1" }
    Shape {
        width: 20; height: 20
        preferredRendererType: Shape.CurveRenderer
        ShapePath {
            fillColor: "#35291e"; strokeColor: "transparent"; strokeWidth: 0
            PathSvg { path: "M5 8 Q9 10 14 7 Q15 11 11 12 Q7 12 5 8 Z" }
        }
    }
    Rectangle { x: 6; y: 9; width: 1; height: 1; radius: 0; color: "#e2d5aa"; antialiasing: true;  }
    Rectangle { x: 8; y: 10; width: 1; height: 1; radius: 0; color: "#e2d5aa"; antialiasing: true;  }
    Rectangle { x: 11; y: 9; width: 1; height: 1; radius: 0; color: "#e2d5aa"; antialiasing: true;  }
    Rectangle { x: 13; y: 8; width: 1; height: 1; radius: 0; color: "#e2d5aa"; antialiasing: true;  }
    Rectangle { x: 10; y: 10; width: 2; height: 3; radius: 0.7; color: "#bb6e64"; antialiasing: true; visible: art.happy; objectName: "hoverMark" }
    Rectangle { x: 10; y: 14; width: 2; height: 2; radius: 1; color: "#646b36"; antialiasing: true;  }
    Rectangle { x: 10; y: 14; width: 1; height: 1; radius: 0.5; color: "#3e4528"; antialiasing: true;  }
    Rectangle { x: 6; y: 12; width: 2; height: 2; radius: 1; color: "#d2bb69"; antialiasing: true;  }
    Rectangle { x: 14; y: 15; width: 1; height: 1; radius: 0.5; color: "#d2bb69"; antialiasing: true;  }
    Rectangle { x: 5; y: 18; width: 1; height: 1; radius: 0; color: "#d2c99c"; antialiasing: true;  }
    Rectangle { x: 14; y: 18; width: 1; height: 1; radius: 0; color: "#d2c99c"; antialiasing: true;  }
}
