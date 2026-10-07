import fs from "fs";
const g = JSON.parse(fs.readFileSync("gen-icons.json", "utf8"));
const src = fs.readFileSync("src/App.tsx", "utf8");

function bbox(d) {
  const toks = d.match(/[MLHVCZmlhvcz]|-?\d*\.?\d+/g) ?? [];
  let i=0,x=0,y=0,sx=0,sy=0,cmd="";
  let minX=Infinity,minY=Infinity,maxX=-Infinity,maxY=-Infinity;
  const hit=(px,py)=>{if(px<minX)minX=px;if(px>maxX)maxX=px;if(py<minY)minY=py;if(py>maxY)maxY=py;};
  const num=()=>parseFloat(toks[i++]);
  const cubic=(x0,y0,x1,y1,x2,y2,x3,y3)=>{for(let t=0;t<=1.0001;t+=1/60){const u=1-t;
    hit(u*u*u*x0+3*u*u*t*x1+3*u*t*t*x2+t*t*t*x3, u*u*u*y0+3*u*u*t*y1+3*u*t*t*y2+t*t*t*y3);}};
  while(i<toks.length){
    if(/[A-Za-z]/.test(toks[i]))cmd=toks[i++];
    if(cmd==="M"){x=num();y=num();sx=x;sy=y;hit(x,y);cmd="L";}
    else if(cmd==="L"){x=num();y=num();hit(x,y);}
    else if(cmd==="H"){x=num();hit(x,y);}
    else if(cmd==="V"){y=num();hit(x,y);}
    else if(cmd==="C"){const a=num(),b=num(),c=num(),dd=num(),e=num(),f=num();cubic(x,y,a,b,c,dd,e,f);x=e;y=f;}
    else if(cmd==="Z"||cmd==="z"){x=sx;y=sy;} else {i++;}
  }
  return {minX,minY,maxX,maxY};
}

let bad = 0;
for (const [name, o] of Object.entries(g)) {
  const [a,,,d,e,f] = o.matrix.match(/-?\d*\.?\d+/g).map(Number);
  const b = bbox(o.d);
  // apply matrix(a 0 0 d e f) to the path bbox corners
  const xs = [a*b.minX+e, a*b.maxX+e], ys = [d*b.minY+f, d*b.maxY+f];
  const x0=Math.min(...xs), x1=Math.max(...xs), y0=Math.min(...ys), y1=Math.max(...ys);
  const w=x1-x0, h=y1-y0, cx=(x0+x1)/2, cy=(y0+y1)/2;
  const inBox = x0 >= -0.01 && y0 >= -0.01 && x1 <= 24.01 && y1 <= 24.01;
  const centred = Math.abs(cx-12) < 0.1 && Math.abs(cy-12) < 0.1;
  const height14 = Math.abs(h-14) < 0.15 || Math.abs(w-17) < 0.15;
  const okAll = inBox && centred && height14;
  if (!okAll) bad++;
  console.log(`  ${name.padEnd(9)} ${w.toFixed(1).padStart(5)} x ${h.toFixed(1).padStart(5)}  centre(${cx.toFixed(2)}, ${cy.toFixed(2)})  inside24=${inBox}  ${okAll?"OK":"<-- PROBLEM"}`);
  // and confirm the exact path shipped in App.tsx
  if (!src.includes(o.d)) { console.log(`     path NOT found in App.tsx`); bad++; }
  if (!src.includes(o.matrix)) { console.log(`     matrix NOT found in App.tsx`); bad++; }
}
console.log(bad ? `\n${bad} problems` : "\nall icons normalised, centred and within the viewBox");
process.exit(bad?1:0);
