// xterm 6.0.0's compiled KeyCodeUtils namespace inherits a frozen toString.
// Use a null-prototype namespace, not an unfrozen Object.prototype. Public API and
// terminal rendering are unchanged; fail closed if the locked vendor code moves.
export function frozenXterm(source) {
  const anchor='KeyCodeUtils=d={}';
  if(source.split(anchor).length!==2) throw Error('Locked xterm 6 KeyCodeUtils compatibility anchor changed');
  return source.replace(anchor,'KeyCodeUtils=d=Object.create(null)');
}
export default function frozenXtermPlugin(){
  return {name:'phase7b-frozen-xterm',enforce:'pre',transform(code,id){
    if(id.split('?')[0].replaceAll('\\','/').endsWith('/@xterm/xterm/lib/xterm.js')) return {code:frozenXterm(code),map:null};
  }};
}
